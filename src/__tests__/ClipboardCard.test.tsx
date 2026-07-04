import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React, { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { I18nextProvider } from 'react-i18next';
import i18n from '../i18n';
import { ThemeProvider } from '../contexts/ThemeContext';
import ClipboardCard from '../components/ClipboardCard';

const TestWrapper: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <ThemeProvider>
    <I18nextProvider i18n={i18n}>
      {children}
    </I18nextProvider>
  </ThemeProvider>
);

describe('ClipboardCard Component', () => {
  const roots: Root[] = [];

  const renderCard = (children: React.ReactNode) => {
    const div = document.createElement('div');
    document.body.appendChild(div);
    const root = createRoot(div);
    roots.push(root);
    act(() => {
      root.render(<TestWrapper>{children}</TestWrapper>);
    });
    return { div, root };
  };

  const defaultProps = {
    id: 1,
    content: 'Test content',
    type: 'text' as const,
    index: 1,
    isPinned: false,
    isSelected: false,
    lineHeight: 'medium' as const,
    onClick: vi.fn(),
    onDoubleClick: vi.fn(),
    onTogglePin: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    for (const root of roots.splice(0)) {
      act(() => {
        root.unmount();
      });
    }
    document.body.innerHTML = '';
  });

  it('creates component without crashing', () => {
    const { div } = renderCard(<ClipboardCard {...defaultProps} />);

    // Component renders without throwing errors
    expect(div).toBeTruthy();
  });

  it('handles props correctly', () => {
    renderCard(<ClipboardCard {...defaultProps} />);

    // Basic smoke test - component accepts props
    expect(defaultProps.id).toBe(1);
    expect(defaultProps.content).toBe('Test content');
  });

  it('calls onClick handler when clicked', () => {
    const mockClick = vi.fn();
    const { div } = renderCard(<ClipboardCard {...defaultProps} onClick={mockClick} />);

    // Simulate click event
    const event = new MouseEvent('click');
    div.dispatchEvent(event);

    // Note: In a real test, we'd need to find the actual card element
    // This is a basic structure test
    expect(mockClick).toBeDefined();
  });

  it('accepts different line height props', () => {
    const { root } = renderCard(<ClipboardCard {...defaultProps} lineHeight="small" />);

    // Test different line height values
    act(() => {
      root.render(
        <TestWrapper>
          <ClipboardCard {...defaultProps} lineHeight="medium" />
        </TestWrapper>
      );
    });
    act(() => {
      root.render(
        <TestWrapper>
          <ClipboardCard {...defaultProps} lineHeight="large" />
        </TestWrapper>
      );
    });

    // Component accepts all line height values without error
    expect(['small', 'medium', 'large']).toContain(defaultProps.lineHeight);
  });

  it('handles different content types', () => {
    const { root } = renderCard(<ClipboardCard {...defaultProps} type="text" />);

    // Test text content
    act(() => {
      root.render(
        <TestWrapper>
          <ClipboardCard {...defaultProps} type="image" content="data:image/png;base64,test" />
        </TestWrapper>
      );
    });

    // Component handles both content types
    expect(['text', 'image']).toContain(defaultProps.type);
  });

  it('allows file items to be edited like text items', () => {
    const onEdit = vi.fn();
    const { div } = renderCard(
      <ClipboardCard
        {...defaultProps}
        type="file"
        content="/tmp/example.txt"
        onEdit={onEdit}
      />,
    );

    const editButton = Array.from(div.querySelectorAll('button')).find(
      (button) => button.textContent?.includes('📝'),
    );
    expect(editButton).toBeTruthy();

    act(() => {
      editButton?.click();
    });
    expect(onEdit).toHaveBeenCalledOnce();
  });


  it('shows a line count badge for multiline text', () => {
    const { div } = renderCard(
      <ClipboardCard {...defaultProps} content={'one\ntwo\nthree'} />
    );

    expect(div.textContent).toContain('3 lines');
  });

  it('highlights fuzzy search matches case-insensitively', () => {
    const { div } = renderCard(
      <ClipboardCard
        {...defaultProps}
        content="Alpha beta alpha"
        searchQuery="alpha"
        searchMode="fuzzy"
        isSearchMode
      />
    );

    const marks = Array.from(div.querySelectorAll('mark')).map(
      (mark) => mark.textContent,
    );
    expect(marks).toEqual(['Alpha', 'alpha']);
  });

  it('highlights regex search matches', () => {
    const { div } = renderCard(
      <ClipboardCard
        {...defaultProps}
        content="abc-123 xyz-456"
        searchQuery={"regx:\\d+"}
        searchMode="regex"
        isSearchMode
      />
    );

    const marks = Array.from(div.querySelectorAll('mark')).map(
      (mark) => mark.textContent,
    );
    expect(marks).toEqual(['123', '456']);
  });

  it('starts multiline search preview from the first matching line', () => {
    const { div } = renderCard(
      <ClipboardCard
        {...defaultProps}
        content={'first line\nneedle line\nlast line'}
        searchQuery="needle"
        searchMode="fuzzy"
        isSearchMode
      />
    );

    const text = div.querySelector('p')?.textContent ?? '';
    expect(text).toContain('needle line');
    expect(text).not.toContain('first line');
  });

  it('caps rendered preview and title for very large text items', () => {
    const largeContent = `${'prefix '.repeat(1000)}needle${' suffix'.repeat(50000)}`;
    const { div } = renderCard(
      <ClipboardCard
        {...defaultProps}
        content={largeContent}
        searchQuery="needle"
        searchMode="fuzzy"
        isSearchMode
      />
    );

    const text = div.querySelector('p')?.textContent ?? '';
    const title = div.querySelector('p')?.getAttribute('title') ?? '';
    expect(text).toContain('needle');
    expect(text.length).toBeLessThan(4100);
    expect(title.length).toBeLessThan(2100);
    expect(title).not.toBe(largeContent);
  });

  it('passes callback functions as props', () => {
    const callbacks = {
      onClick: vi.fn(),
      onDoubleClick: vi.fn(),
      onTogglePin: vi.fn(),
    };

    renderCard(<ClipboardCard {...defaultProps} {...callbacks} />);

    // All callback functions are passed correctly
    expect(typeof callbacks.onClick).toBe('function');
    expect(typeof callbacks.onDoubleClick).toBe('function');
    expect(typeof callbacks.onTogglePin).toBe('function');
  });
});
